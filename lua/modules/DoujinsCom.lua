local dirurl = '/list?type=&sortType=DATE_CREATE'
local perpage = 60

function getinfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x=CreateTXQuery(HTTP.Document)
		if MANGAINFO.Title == '' then
			MANGAINFO.Title = x.XPathString('//div[@class="folder-title"]/a[last()]')
		end
		MANGAINFO.CoverLink = x.XPathString('(//img[@class="doujin"])[1]/@data-thumb')
		MANGAINFO.Authors=x.XPathStringAll('//div[@class="gallery-artist"]/a')
		MANGAINFO.Artists=x.XPathStringAll('//div[@class="gallery-artist"]/a')
		MANGAINFO.Genres=x.XPathStringAll('//li[@class="tag-area"]/a')
		MANGAINFO.ChapterLinks.Add(MANGAINFO.URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function getpagenumber()
	TASK.PageLinks.Clear()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		local x=CreateTXQuery(HTTP.Document)
		x.XPathStringAll('//img[@class="doujin"]/@data-file', TASK.PageLinks)
	else
		return false
	end
	return true
end

function today()
	local now = os.date('*t');
	return os.time({year=now.year, month=now.month, day=now.day})
end

local endDate = os.time({year=2007, month=9, day=30})
local step = 30 * 24 * 60 * 60

function getdirectorypagenumber()
	PAGENUMBER = math.ceil((today() - endDate) / step)
	return no_error
end

function getnameandlink()
	local to = today() - tonumber(URL) * step
	local from = to - step + 1
	if from < endDate then return no_error; end
	if HTTP.XHR(MODULE.RootURL .. string.format('/folders?start=%d&end=%d', from, to)) then
		local x = CreateTXQuery(HTTP.Document)
		local v = x.XPath('json(*).folders()')
		for i = 1, v.Count do
			local v1 = v.Get(i)
			LINKS.Add(x.XPathString('link', v1))
			NAMES.Add(x.XPathString('name', v1))
		end
		return no_error
	else
		return net_problem
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID = 'f0e671e36a9b47109e6ba1064a23eb0b'
	m.Name = 'DoujinsCom'
	m.RootURL = 'https://doujins.com'
	m.Category = 'H-Sites'
	m.LastUpdated='May 14, 2018'
	m.SortedList = true
	m.OnGetInfo='getinfo'
	m.OnGetPageNumber='getpagenumber'
	m.OnGetNameAndLink='getnameandlink'
	m.OnGetDirectoryPageNumber = 'getdirectorypagenumber'
end