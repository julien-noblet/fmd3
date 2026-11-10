function GetInfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL,URL)
	if HTTP.GET(MANGAINFO.URL) then
		x=CreateTXQuery(HTTP.Document)
		MANGAINFO.Title=x.XPathString('//title/substring-before(.," | ")')
		MANGAINFO.CoverLink=x.XPathString('//meta[@property="og:image"]/@content')

		MANGAINFO.ChapterLinks.Add(URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function TaskStart()
	TASK.PageLinks.Clear()
	TASK.PageContainerLinks.Clear()
	TASK.PageNumber=0
	return true
end;

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL,URL)) then
		x=CreateTXQuery(HTTP.Document)
		x.XPathStringAll('//*[@id="book_data_area"]/input[@data-key="imageCodes"]/@value', TASK.PageLinks)
		TASK.PageContainerLinks.Add(URL:gsub('^/',''))
		-- TASK.PageContainerLinks.Add(x.XPathString('//*[@id="book_data_area"]/input[@data-key="isbn"]/@value'))
		TASK.PageContainerLinks.Add(x.XPathString('//*[@id="book_data_area"]/input[@data-key="vsid"]/@value'))
		return true
	else
		return false
	end
end

function DownloadImage()
	if WORKID == 0 then
		HTTP.Headers.Values['Referer']=' '..MODULE.RootURL..'/'..TASK.PageContainerLinks[1]
	else
		HTTP.Headers.Values['Referer']=' '..MODULE.RootURL..'/'..TASK.PageContainerLinks[1]..'?page='..tostring(WorkId)
	end

	if HTTP.POST(MODULE.RootURL..'/imgDeliver?jan_cd='..TASK.PageContainerLinks[0],'base64=1&vsid='..TASK.PageContainerLinks[1]..'&trgCode='..TASK.PageLinks[WORKID]) then
		local crypto = require 'fmd.crypto'
		return crypto.DecodeBase64(HTTP.Document.ToString())
	else
		return false
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID = '00d9f45f1f174058b5eb8a72c0b5d7c6'
	-- m.Category='Raw'
	m.Name='Shogakukan'
	m.RootURL='https://shogakukan.tameshiyo.me'
	m.OnGetInfo='GetInfo'
	m.OnTaskStart='TaskStart'
	m.OnGetPageNumber='GetPageNumber'
	m.OnDownloadImage='DownloadImage'
end
